// Phase 219.E / 235.A — `<nros/main.hpp>` Entry-pkg header.
//
// The cmake fn `nano_ros_entry(LAUNCH "<bringup>:<file>.launch.xml")`
// drives the per-Entry-pkg codegen via `nros codegen entry --lang cpp`,
// then appends the generated TU to the executable target's sources.
// The generated TU has the canonical body — `int main()` + the
// `nros::board::NativeBoard::run(lambda)` boot stub + the per-Node
// register-call sequence.
//
// This header provides two ingredients the generated TU needs:
//
//   1. `NROS_MAIN(<Board>, "<bringup>:<file>.launch.xml")` — empty-
//      expansion macro the user's own TU may carry as a doc/IDE hint
//      (parallels Rust's `nros::main!(launch = "…")`). It expands to
//      a sentinel symbol the cmake fn can detect with
//      `target_compile_definitions` to avoid double-emit when the
//      user wrote it. The actual code generation happens via the CLI;
//      the macro itself is declarative.
//
//   2. `nros::board::<Board>::run(register_fn)` — the Board adapter shim
//      the generated TU calls. Owns the
//      `nros::init() → register_fn(context) → spin → nros::shutdown()`
//      lifecycle so the generated TU stays one declarative lambda.
//
// Two Board adapters ship (Phase 235.B):
//   * `nros::board::NativeBoard` — host/POSIX; runtime domain + locator.
//   * `nros::board::ZephyrBoard` — embedded Zephyr; compile-time domain
//     id, network-wait hook, cooperative spin. Selected through the
//     Phase 215 `nano_ros_use_board(<name>)` import (board.cmake feeds
//     the default RMW + `west` runner). cf. RFC-0032 §8a.
//
// Both adapters share the SAME `detail::EntryNodeRuntime` ops + arena —
// only the boot lifecycle differs (init / network-wait / cooperative
// yield). The op set is factored into `detail::entry_register` +
// `detail::entry_node_context_ops` so neither board duplicates it.

#ifndef NROS_CPP_MAIN_HPP
#define NROS_CPP_MAIN_HPP

#include "nros/nros.hpp"
/* Phase 274.W2 — nros_native_tier_spec_t + nros_board_native_run_tiers
 * (C ABI board runner for multi-tier native entries). */
#include "nros/main.h"

#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
#include <cstdio>  // printf — Phase 238.B listener readiness / received-sample lines
#include <cstdlib> // getenv — Phase 235.A bounded-spin ($NROS_ENTRY_SPIN_MS)
#endif

// Phase 235.B — the embedded (Zephyr) Board adapter is cooperatively
// scheduled, so the shared Entry spin loop yields each tick (`k_yield()`)
// to let the network stack + peer threads run. Pull the kernel header
// only on Zephyr; the native path keeps the loop dependency-free.
#ifdef __ZEPHYR__
#include <zephyr/kernel.h>
#endif

// Phase 235.A — fixed-capacity arena dimensions for the native
// NodeContext runtime. The runtime is `no_std` / heap-free (mirrors the
// nros-cpp inline-storage discipline), so it pre-sizes its node + entity
// tables at compile time. Override before including this header if an
// Entry pkg declares more than the defaults.
#ifndef NROS_ENTRY_MAX_NODES
#define NROS_ENTRY_MAX_NODES 8
#endif
#ifndef NROS_ENTRY_MAX_ENTITIES
#define NROS_ENTRY_MAX_ENTITIES 24
#endif

namespace nros {
namespace board {
namespace detail {

// ----- parse helper (no STL) ----------------------------------------

inline uint32_t entry_parse_u32(const char* s) {
    uint32_t v = 0;
    if (s == nullptr) return 0;
    for (; *s >= '0' && *s <= '9'; ++s) {
        v = v * 10 + static_cast<uint32_t>(*s - '0');
    }
    return v;
}

// Phase 235.B — per-tick cooperative yield in the shared component spin loop.
// Native relies on `spin_once`'s blocking I/O wait for pacing (no-op here);
// Zephyr is cooperatively scheduled, so each tick must `k_yield()` to release
// the CPU to the network stack + peer threads. Shared by every Board's
// `run_components` via `detail::component_spin_loop`.
inline void entry_tick_yield() {
#ifdef __ZEPHYR__
    k_yield();
#endif
}

/// Phase 240.2 (RFC-0043) — the **real-executor** spin loop, shared by every
/// Board's `run_components`. Unlike `EntryNodeRuntime::spin`, it runs NO
/// synthesizing interpreter: the user's components already registered their real
/// callbacks on the executor during their `configure`, so this just pumps
/// `spin_once` (which dispatches them) until `nros::ok()` flips false, or for
/// `$NROS_ENTRY_SPIN_MS` ms when set (the bounded external-observer test path).
/// Returns the first non-zero `spin_once` code, else 0.
inline int32_t component_spin_loop() {
    uint32_t bound_ms = 0;
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
    const char* env = ::std::getenv("NROS_ENTRY_SPIN_MS");
    if (env != nullptr && env[0] != '\0') {
        bound_ms = entry_parse_u32(env);
    }
#endif
    const uint64_t start_ns = nros_cpp_time_ns();
    for (;;) {
        if (bound_ms == 0 && !::nros::ok()) break;
        ::nros::Result last = ::nros::spin_once(10);
        if (!last.ok()) return static_cast<int32_t>(last.raw());
        if (bound_ms != 0) {
            const uint64_t elapsed_ms = (nros_cpp_time_ns() - start_ns) / 1000000ull;
            if (elapsed_ms >= bound_ms) break;
            if (!::nros::ok()) break;
        }
        entry_tick_yield();
    }
    return 0;
}

} // namespace detail

// Phase 235.B — weak network-readiness hook for embedded Board adapters.
//
// Default: no-op. The canonical in-tree Zephyr path auto-brings-up
// networking at boot (`CONFIG_NET_CONFIG_AUTO_INIT` — static IP / DHCP),
// so `ZephyrBoard::run` needs no explicit wait. A board crate or Entry app
// that must block until the link / DHCP lease is ready (e.g. ASI's
// `configure_network()` prologue) provides a STRONG definition of this
// symbol, which the linker prefers over the weak default. Mirrors the
// weak-default discipline already used for `nros_app_register_backends`.
extern "C" {
#if defined(__GNUC__) || defined(__clang__)
__attribute__((weak)) void nros_board_network_wait(void) {}
#else
void nros_board_network_wait(void);
#endif
}

/// Phase 274.W2 (RFC-0015 Model 1) — per-tier spec for
/// `NativeBoard::run_tiers`. Layout mirrors `nros_native_tier_spec_t` in
/// `<nros/nros_cpp_ffi.h>` (included transitively via `<nros/nros.hpp>`);
/// the `run_tiers` static method casts between the two.
///
/// `name`           — tier name (null-terminated), informational.
/// `groups`         — array of `n_groups` null-terminated callback-group
///                    names; NULL / 0 means wildcard (accept all groups).
/// `n_groups`       — number of entries in `groups`.
/// `priority`       — raw POSIX nice level adjustment (advisory on Linux).
/// `stack_bytes`    — informational on native (`std::thread` manages the stack).
/// `spin_period_us` — sleep between `spin_once` calls; 0 → 1 ms floor.
/// `setup`          — called once on the tier thread (after `set_active_groups`)
///                    with the tier executor handle; returns 0 on success.
struct NativeTierSpec {
    const char* name;
    const char** groups;
    size_t n_groups;
    int64_t priority;
    size_t stack_bytes;
    uint64_t spin_period_us;
    int32_t (*setup)(void* executor);
};

class NativeBoard {
  public:
    /// Phase 266 (W6) — named variant: `session_name` sets the primary session /
    /// node name visible via `ros2 node list` (the #98 fix for C++ entries). NULL
    /// or empty → falls back to `"node"` (the unified default). The generated
    /// typed C++ entry (emitted by `nros codegen entry --lang cpp --typed`) calls
    /// this overload, passing `nros_boot_config_node_name(&NROS_BOOT_CONFIG)`.
    template <typename Setup>
    static int32_t run_components(const char* session_name, Setup&& setup) {
        const char* sn =
            (session_name != nullptr && session_name[0] != '\0') ? session_name : "node";
        // Phase 266: env overlay (NROS_LOCATOR / ROS_DOMAIN_ID) applies via the
        // 3-arg init — null locator and 0 domain_id both trigger the env fallback.
        nros::Result r = nros::init(nullptr, 0, sn);
        if (!r.ok()) return static_cast<int32_t>(r.raw());
        int32_t rc = setup();
        if (rc != 0) {
            nros::shutdown();
            return rc;
        }
        int32_t sc = detail::component_spin_loop();
        nros::shutdown();
        return sc;
    }

    /// Phase 240.2 (RFC-0043) — real-executor entry. `setup` (invoked once after
    /// init, before the spin loop) constructs + `configure`s the user's
    /// component objects, which bind their real callbacks on the executor.
    /// `setup` returns 0 on success. No `EntryNodeRuntime` / synthesis.
    /// Phase 266: delegates to the named overload with "node" (the unified default).
    template <typename Setup> static int32_t run_components(Setup&& setup) {
        return run_components("node", static_cast<Setup&&>(setup));
    }

    /// Phase 274.W2 (RFC-0015 Model 1) — multi-tier native entry.
    ///
    /// Delegates to the C-ABI seam `nros_board_native_run_tiers` which opens
    /// ONE RMW session on the boot thread, spawns one `std::thread` per
    /// non-boot tier (each with a **borrowed** executor sharing the one session),
    /// and runs per-tier `setup` + `spin_once` loops at their declared periods.
    ///
    /// `session_name` sets the primary session / node name; NULL or empty →
    /// `"node"` (the unified default). `tiers` must be a non-null array of
    /// `n_tiers` `NativeTierSpec` entries sorted highest-priority-first (the
    /// codegen emitter produces them in that order). Returns 0 on clean exit
    /// (NROS_ENTRY_SPIN_MS elapsed) or the first non-zero setup / spin code.
    static int32_t run_tiers(const char* session_name, const NativeTierSpec* tiers,
                             size_t n_tiers) {
        // NativeTierSpec and nros_native_tier_spec_t have identical layout by
        // construction (same field order, same types, same ABI). The cast is
        // safe; both structs are plain C-compatible aggregates.
        return ::nros_board_native_run_tiers(
            session_name, reinterpret_cast<const ::nros_native_tier_spec_t*>(tiers), n_tiers);
    }
};

/// Phase 235.B — embedded (Zephyr) board adapter, the `Board::run()`
/// sibling to `NativeBoard` (RFC-0032 §8a).
///
/// **Board granularity decision (RFC-0032 §8a open item).** ONE
/// metadata-driven `ZephyrBoard`, not per-board types (`FvpAemv8rBoard`,
/// …). Everything that varies per board — the Zephyr `BOARD` id, the DTS
/// overlay, the default RMW, the `west` runner — is already supplied by
/// the Phase 215 `nano_ros_use_board(<name>)` cmake import + Kconfig at
/// *build* time, so the C++ adapter has nothing board-specific left to
/// specialize at the source level. A per-board C++ type would only
/// duplicate this lifecycle with no behavioural difference. (RFC-0032
/// already leaned this way: "single + metadata-driven".)
///
/// Lifecycle (mirrors ASI `actuation_module/src/main.cpp`):
///   `nros::init(domain) → network-wait → register_fn → spin → shutdown`.
///
/// The runtime ops + arena are the **same** `detail::EntryNodeRuntime`
/// machinery `NativeBoard` uses — only the lifecycle differs:
///   * domain id is **compile-time** (Kconfig `CONFIG_NROS_*_DOMAIN_ID`),
///     never a runtime env (CLAUDE.md embedded domain-id rule);
///   * a `nros_board_network_wait()` hook runs before init so a board /
///     app can block for DHCP / link-up (default no-op — Zephyr
///     auto-brings-up networking);
///   * the spin loop yields cooperatively each tick (`entry_tick_yield`
///     → `k_yield()`).
// Phase 244.C2 enabler — compile-time connect locator for Zephyr (+ NuttX,
// which reuses this macro). Defaults to the Kconfig `CONFIG_NROS_ZENOH_LOCATOR`
// when the board sets one (the e2e gate threads
// `CONFIG_NROS_ZENOH_LOCATOR=tcp/127.0.0.1:<port>` per fixture), else `""`
// (backend discovery — the in-tree FVP Cyclone path). The typed carrier may
// also bake a literal by defining `NROS_ENTRY_LOCATOR` before this header.
#ifndef NROS_ENTRY_LOCATOR
#if defined(CONFIG_NROS_ZENOH_LOCATOR)
#define NROS_ENTRY_LOCATOR CONFIG_NROS_ZENOH_LOCATOR
#else
#define NROS_ENTRY_LOCATOR ""
#endif
#endif

class ZephyrBoard {
  public:
    /// Compile-time domain id (CLAUDE.md embedded rule — NOT a runtime
    /// env). Cyclone keys off `CONFIG_NROS_CYCLONE_DOMAIN_ID` when present
    /// (matches ASI), else the generic `CONFIG_NROS_DOMAIN_ID`. Override
    /// by defining `NROS_ENTRY_DOMAIN_ID` before including this header.
#ifndef NROS_ENTRY_DOMAIN_ID
#if defined(NROS_RMW_CYCLONEDDS) && defined(CONFIG_NROS_CYCLONE_DOMAIN_ID)
#define NROS_ENTRY_DOMAIN_ID CONFIG_NROS_CYCLONE_DOMAIN_ID
#elif defined(CONFIG_NROS_DOMAIN_ID)
#define NROS_ENTRY_DOMAIN_ID CONFIG_NROS_DOMAIN_ID
#else
#define NROS_ENTRY_DOMAIN_ID 0
#endif
#endif

    /// Phase 266 (W6) — 3-arg named overload: explicit locator + session name.
    /// `session_name` sets the primary session / node name (`ros2 node list`).
    /// NULL or empty → `"node"`. The generated C++ entry calls this with
    /// `NROS_ENTRY_LOCATOR` and `nros_boot_config_node_name(&NROS_BOOT_CONFIG)`.
    template <typename Setup>
    static int32_t run_components(const char* locator, const char* session_name, Setup&& setup) {
        nros_board_network_wait();
        const char* sn =
            (session_name != nullptr && session_name[0] != '\0') ? session_name : "node";
        nros::Result r = nros::init(locator, static_cast<uint8_t>(NROS_ENTRY_DOMAIN_ID), sn);
        if (!r.ok()) return static_cast<int32_t>(r.raw());
        int32_t rc = setup();
        if (rc != 0) {
            nros::shutdown();
            return rc;
        }
        int32_t sc = detail::component_spin_loop();
        nros::shutdown();
        return sc;
    }

    /// Phase 240.2 (RFC-0043) — real-executor entry (Zephyr lifecycle), explicit
    /// connect locator (Phase 244.C2). `setup` constructs + `configure`s nodes.
    /// Phase 266: delegates to 3-arg overload with "node" default session name.
    template <typename Setup> static int32_t run_components(const char* locator, Setup&& setup) {
        return run_components(locator, "node", static_cast<Setup&&>(setup));
    }

    /// Locator-less overload — uses the compile-time `NROS_ENTRY_LOCATOR`.
    /// Phase 266: delegates to 3-arg overload with "node" default session name.
    template <typename Setup> static int32_t run_components(Setup&& setup) {
        return run_components(NROS_ENTRY_LOCATOR, "node", static_cast<Setup&&>(setup));
    }
};

/// Phase 238 — embedded NuttX board adapter, sibling to `ZephyrBoard`.
///
/// NuttX brings up `eth0` (virtio-net) during kernel boot **before** the
/// app entry runs (see `nros-board-nuttx-qemu-arm::entry_212n` —
/// "NuttX brings up eth0 during kernel boot before main"), so — like the
/// Zephyr `CONFIG_NET_CONFIG_AUTO_INIT` path — no explicit network wait is
/// needed; the weak `nros_board_network_wait()` default no-op is correct.
///
/// The runtime ops + arena are the **same** `detail::EntryNodeRuntime`
/// machinery `NativeBoard` / `ZephyrBoard` use — only the lifecycle differs:
///   * domain id is **compile-time** (`NROS_ENTRY_DOMAIN_ID`, fed from the
///     example's `nano_ros_deploy(DOMAIN_ID …)` → `CONFIG_NROS_DOMAIN_ID`),
///     never a runtime env (CLAUDE.md embedded domain-id rule);
///   * the cooperative `entry_tick_yield()` is a no-op on NuttX (the
///     preemptive scheduler + `spin_once`'s `z_sleep_ms` pacing release the
///     CPU to the zenoh-pico read/lease tasks — see CLAUDE.md
///     `zpico_spin_once` note).
///
/// The bootable ELF *is* the NuttX kernel: the generated entry TU is
/// compiled as `APP_MAIN_CPP` and linked into the kernel by the cargo
/// `nros-nuttx-ffi` build (`nuttx_ffi_build.rs`), driven from the carrier
/// cmake (`nano_ros_node_register` NuttX branch → `nros_platform_link_app`).
// Phase 238 — compile-time default connect locator for the locator-less
// `NuttxBoard::run(lambda)` overload. The carrier normally bakes the real
// locator into the generated entry TU and calls the 2-arg overload; this
// default only applies if a hand-written entry uses the 1-arg form.
#ifndef NROS_ENTRY_LOCATOR
#define NROS_ENTRY_LOCATOR ""
#endif

class NuttxBoard {
  public:
    /// Phase 266 (W6) — 3-arg named overload: explicit locator + session name.
    /// `session_name` sets the primary session / node name (`ros2 node list`).
    /// NULL or empty → `"node"`. The generated C++ entry calls this with
    /// `NROS_ENTRY_LOCATOR` and `nros_boot_config_node_name(&NROS_BOOT_CONFIG)`.
    template <typename Setup>
    static int32_t run_components(const char* locator, const char* session_name, Setup&& setup) {
        nros_board_network_wait();
        const char* sn =
            (session_name != nullptr && session_name[0] != '\0') ? session_name : "node";
        nros::Result r = nros::init(locator, static_cast<uint8_t>(NROS_ENTRY_DOMAIN_ID), sn);
        if (!r.ok()) return static_cast<int32_t>(r.raw());
        int32_t rc = setup();
        if (rc != 0) {
            nros::shutdown();
            return rc;
        }
        int32_t sc = detail::component_spin_loop();
        nros::shutdown();
        return sc;
    }

    /// Run the Entry-pkg lifecycle on a NuttX board with an explicit
    /// connect `locator`. The bootable-ELF carrier
    /// (`nano_ros_node_register` NuttX branch) bakes the locator into the
    /// generated entry TU (`configure_file` of
    /// `cmake/templates/nuttx_entry_main.cpp.in`) because — unlike Zephyr's
    /// `CONFIG_NET_CONFIG_AUTO_INIT` peer discovery — the QEMU slirp guest
    /// must dial the host zenoh router explicitly (`tcp/10.0.2.2:<port>`),
    /// mirroring the Rust `*_entry` pkg's `[…entry] locator = …` bake.
    /// `locator == ""` falls back to backend discovery.
    /// Phase 240.2 (RFC-0043) — real-executor entry (NuttX lifecycle, explicit
    /// connect locator). Phase 266: delegates to 3-arg overload with "node" default.
    template <typename Setup> static int32_t run_components(const char* locator, Setup&& setup) {
        return run_components(locator, "node", static_cast<Setup&&>(setup));
    }

    /// Locator-less overload — uses the compile-time `NROS_ENTRY_LOCATOR`.
    /// Phase 266: delegates to 3-arg overload with "node" default session name.
    template <typename Setup> static int32_t run_components(Setup&& setup) {
        return run_components(NROS_ENTRY_LOCATOR, "node", static_cast<Setup&&>(setup));
    }
};

/// Phase 246 — Azure RTOS ThreadX board adapter (C/C++ declarative components
/// on threadx-linux + bare-metal riscv64), routing the RFC-0043 typed entry to
/// the real executor.
///
/// Lifecycle-identical to [`NuttxBoard`]: by the time this runs we are
/// **already inside the ThreadX application thread** (the board's C
/// `startup.c::main` called `tx_kernel_enter()` and the app thread dispatches
/// to the typed entry's `app_main`), and NetX Duo is already up — so
/// `run_components` MUST NOT enter the kernel. It brings the nros runtime online
/// and spins the real executor: `network_wait` (weak no-op — NetX is up at
/// boot) → `nros::init` → `setup` (constructs the component + `configure(node)`
/// binds real callbacks) → `detail::component_spin_loop` → `shutdown`.
///
/// Domain id is `NROS_ENTRY_DOMAIN_ID` (embedded: compile-time, never env —
/// CLAUDE.md). (The retired synthesizing-interpreter `run(register_fn)` overload
/// was dropped in phase-246 — RFC-0043 §Retirement.)
class ThreadxBoard {
  public:
    /// Phase 266 (W6) — 3-arg named overload: explicit locator + session name.
    /// `session_name` sets the primary session / node name (`ros2 node list`).
    /// NULL or empty → `"node"`. The generated C++ entry calls this with
    /// `NROS_ENTRY_LOCATOR` and `nros_boot_config_node_name(&NROS_BOOT_CONFIG)`.
    template <typename Setup>
    static int32_t run_components(const char* locator, const char* session_name, Setup&& setup) {
        nros_board_network_wait();
        const char* sn =
            (session_name != nullptr && session_name[0] != '\0') ? session_name : "node";
        nros::Result r = nros::init(locator, static_cast<uint8_t>(NROS_ENTRY_DOMAIN_ID), sn);
        if (!r.ok()) return static_cast<int32_t>(r.raw());
        int32_t rc = setup();
        if (rc != 0) {
            nros::shutdown();
            return rc;
        }
        int32_t sc = detail::component_spin_loop();
        nros::shutdown();
        return sc;
    }

    /// RFC-0043 real-executor entry (ThreadX lifecycle, explicit locator).
    /// Phase 266: delegates to 3-arg overload with "node" default session name.
    template <typename Setup> static int32_t run_components(const char* locator, Setup&& setup) {
        return run_components(locator, "node", static_cast<Setup&&>(setup));
    }

    /// Locator-less overload — uses the compile-time `NROS_ENTRY_LOCATOR`.
    /// Phase 266: delegates to 3-arg overload with "node" default session name.
    template <typename Setup> static int32_t run_components(Setup&& setup) {
        return run_components(NROS_ENTRY_LOCATOR, "node", static_cast<Setup&&>(setup));
    }
};

/// Phase 240.6 (RFC-0043) — FreeRTOS board adapter (C/C++ declarative
/// components on the QEMU MPS2-AN385 + lwIP stack), routing the typed entry to
/// the real executor.
///
/// Lifecycle-identical to [`NuttxBoard`]: by the time this runs we are
/// **already inside the FreeRTOS application task** (the board's C `startup.c`
/// `_start` spawned the `app` task + called `vTaskStartScheduler()`, and that
/// task's `app_task_entry` brought up LAN9118 + lwIP, the network poll task, the
/// log writer, and the zenoh-pico read/lease task config BEFORE dispatching to
/// the typed entry's `app_main`). So the network is up and the kernel is
/// running — `run_components` MUST NOT enter the kernel. It brings the nros
/// runtime online and spins the real executor: `network_wait` (weak no-op — the
/// startup task already waited on the netif) → `nros::init` → `setup` (constructs
/// the component + `configure(node)` binds real callbacks) →
/// `detail::component_spin_loop` → `shutdown`.
///
/// Domain id is `NROS_ENTRY_DOMAIN_ID` (embedded: compile-time, never env —
/// CLAUDE.md). The connect locator is baked into the generated entry TU by the
/// carrier (QEMU slirp guest dials the host zenoh router at `tcp/10.0.2.2:<port>`).
/// TYPED-only: the retired synthesizing-interpreter `run(register_fn)` overload
/// is not provided (RFC-0043 §Retirement), matching [`ThreadxBoard`].
class FreertosBoard {
  public:
    /// Phase 266 (W6) — 3-arg named overload: explicit locator + session name.
    /// `session_name` sets the primary session / node name (`ros2 node list`).
    /// NULL or empty → `"node"`. The generated C++ entry calls this with
    /// `NROS_ENTRY_LOCATOR` and `nros_boot_config_node_name(&NROS_BOOT_CONFIG)`.
    template <typename Setup>
    static int32_t run_components(const char* locator, const char* session_name, Setup&& setup) {
        nros_board_network_wait();
        const char* sn =
            (session_name != nullptr && session_name[0] != '\0') ? session_name : "node";
        nros::Result r = nros::init(locator, static_cast<uint8_t>(NROS_ENTRY_DOMAIN_ID), sn);
        if (!r.ok()) return static_cast<int32_t>(r.raw());
        int32_t rc = setup();
        if (rc != 0) {
            nros::shutdown();
            return rc;
        }
        int32_t sc = detail::component_spin_loop();
        nros::shutdown();
        return sc;
    }

    /// RFC-0043 real-executor entry (FreeRTOS lifecycle, explicit locator).
    /// Phase 266: delegates to 3-arg overload with "node" default session name.
    template <typename Setup> static int32_t run_components(const char* locator, Setup&& setup) {
        return run_components(locator, "node", static_cast<Setup&&>(setup));
    }

    /// Locator-less overload — uses the compile-time `NROS_ENTRY_LOCATOR`.
    /// Phase 266: delegates to 3-arg overload with "node" default session name.
    template <typename Setup> static int32_t run_components(Setup&& setup) {
        return run_components(NROS_ENTRY_LOCATOR, "node", static_cast<Setup&&>(setup));
    }
};

} // namespace board
} // namespace nros

// Phase 219.E — `NROS_MAIN(<Board>, "<launch_spec>")` declarative
// marker. Expands to a sentinel TU-local symbol so the cmake fn can
// detect macro presence (via `target_compile_definitions` or the
// compiled-out-but-still-elf-visible `__nros_entry_macro_present`
// symbol). The Phase 219.D cmake fn body owns the actual codegen —
// the macro is doc/IDE shape only.
//
// Usage:
//
//   #include <nros/main.hpp>
//   NROS_MAIN(::nros::board::NativeBoard, "demo_bringup:system.launch.xml")
//
// Putting it in a user TU is OPTIONAL. The generated TU (emitted by
// `nano_ros_entry(LAUNCH …)`) carries the canonical `int main()` body
// regardless; the macro is purely a hint for tooling and IDEs that
// parse the source.
#define NROS_MAIN(BoardType, LaunchSpec)                                                           \
    extern "C" const unsigned char __nros_entry_macro_present = 1;                                 \
    static_assert(sizeof(LaunchSpec) > 1, "NROS_MAIN: launch spec must be a non-empty literal")

#endif // NROS_CPP_MAIN_HPP
