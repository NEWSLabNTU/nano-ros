// Phase 219.E — `<nros/main.hpp>` Entry-pkg header.
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
//   2. `nros::board::NativeBoard::run(register_fn)` — the Board
//      adapter shim the generated TU calls. Owns the
//      `nros::init() → register_fn(context) → nros::spin() →
//      nros::shutdown()` lifecycle so the generated TU stays one
//      declarative lambda.
//
// Native is the only Board exposed today (Phase 212.L.2 keeps Entry
// pkgs `native`-only at the cmake surface for v1). Embedded boards
// land separately when the embedded C/C++ board family lands
// (cf. Phase 216.D).

#ifndef NROS_CPP_MAIN_HPP
#define NROS_CPP_MAIN_HPP

#include "nros/nros.hpp"
#include "nros/node_pkg.hpp"

namespace nros::board {

/// Phase 219.B — Native (POSIX) board adapter for Entry-pkg generated
/// TUs.
///
/// Owns the init/spin/shutdown ritual; the generated TU supplies the
/// per-Node register sequence as a `NodeRegisterFn`-shaped lambda.
///
/// Today this is a thin host-only adapter — every C/C++ Node pkg's
/// register fn is *descriptive* (records entities into a NodeContext
/// the runtime later instantiates). The Native runtime that turns a
/// NodeContext into running publishers/subscriptions is **NOT
/// supplied here** — that integration sits below the Phase 219
/// "pure orchestration" scope (see phase doc §7). The shim therefore
/// constructs a *recording* NodeContext whose ops are no-ops, invokes
/// the lambda (so any plain register-side logging fires), then enters
/// `nros::spin()` so the executable stays alive for the test
/// harness. Real per-Node publishers / subscriptions arrive when the
/// Native NodeContext runtime lands as a follow-up.
class NativeBoard {
  public:
    /// Run the Entry-pkg lifecycle. `register_fn` is invoked once,
    /// before the spin loop, with a recording NodeContext.
    ///
    /// Returns the first non-zero code from `register_fn` or
    /// `nros::spin()`. `0` on graceful shutdown.
    template <typename Lambda> static int32_t run(Lambda&& register_fn) {
        nros::Result r = nros::init();
        if (!r.ok()) {
            return static_cast<int32_t>(r.raw());
        }

        // Phase 219.B placeholder NodeContext — every op is a no-op
        // until the Native runtime lands. Lets the generated TU
        // exercise the orchestration glue (codegen, mangled symbol
        // resolution, launch-order dispatch) end-to-end without
        // requiring the runtime side.
        static const ::nros::NodeContextOps ops = {
            /* create_node              */ &NativeBoard::noop_create_node,
            /* create_entity            */ &NativeBoard::noop_create_entity,
            /* record_callback_effect   */ &NativeBoard::noop_record_callback_effect,
        };
        ::nros::NodeContext context(nullptr, &ops);

        int32_t rc = register_fn(&context);
        if (rc != 0) {
            nros::shutdown();
            return rc;
        }

        // Spin until `nros::ok()` flips false (SIGINT handler,
        // explicit `nros::shutdown()`, or transport error). Mirrors
        // the rclcpp `rclcpp::spin(node)` pattern.
        nros::Result spin_r = nros::spin();
        nros::shutdown();
        return static_cast<int32_t>(spin_r.raw());
    }

  private:
    static int32_t noop_create_node(void* /*user*/, const char* /*stable_id*/,
                                    const ::nros::NodeOptions* /*opts*/,
                                    ::nros::DeclaredNode* /*out*/) {
        return 0;
    }
    static int32_t noop_create_entity(void* /*user*/, const void* /*desc*/) { return 0; }
    static int32_t noop_record_callback_effect(void* /*user*/, const char* /*cb_id*/,
                                               ::nros::CallbackEffectKind /*kind*/,
                                               const char* /*entity_id*/) {
        return 0;
    }
};

} // namespace nros::board

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
