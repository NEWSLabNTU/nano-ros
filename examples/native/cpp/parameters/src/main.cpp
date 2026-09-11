/// @file main.cpp
/// @brief C++ parameters example — declare / get / set on THE parameter store.
///
/// phase-426 W4. This example used to build an `nros::ParameterServer<8>` on
/// the stack and exercise it. That class was a SECOND parameter store: the six
/// `rcl_interfaces/srv/*` servers read the executor's `nros_params` table, so
/// every parameter this file declared was invisible to `ros2 param get` — the
/// exact defect phase-426 exists to remove, shipped in the example a user
/// copies out. The class is gone; this is the same demo against the one store.
///
/// What that changed, and it is the point rather than a detail: parameters
/// belong to a NODE, so the example opens a session and a node instead of
/// being a self-contained `main()`. Run it and, while it spins, ask ROS 2:
///
/// ```console
/// $ NROS_SPIN_MS=60000 ./cpp_parameters &
/// $ ros2 param list /cpp_parameters
/// $ ros2 param get  /cpp_parameters ctrl_period
/// $ ros2 param set  /cpp_parameters ctrl_period 0.25
/// ```
///
/// (Add `--no-daemon` if a `ros2` daemon from an earlier session is still
/// holding a stale graph — it caches across domains and answers "Node not
/// found" for a node that is right there.)
///
/// Scalars go through `rclcpp::Node::declare_parameter<T>` / `get_parameter<T>`
/// / `set_parameter<T>`; the sequence parameter goes through the same methods
/// with an `nros::Seq<double, N>` value, which is the freestanding array form
/// (a `-nostdinc++` board has no `std::vector`) and which now reaches the store
/// like every scalar. The example exits 0 only when every roundtrip passes;
/// a non-zero exit code encodes which assertion failed. Consumed by the
/// `parameters_roundtrip` test.

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>

// Route NROS_TRY_RET through fprintf (we have stdio).
#define NROS_TRY_LOG(file, line, expr, ret)                                                        \
    std::fprintf(stderr, "[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/nros.hpp>
#include <nros/nros_cpp_ffi.h>
#include <nros/parameter.hpp>

namespace {

/// How long to keep spinning after the roundtrip, so the six parameter
/// services this node published can actually be asked something. 0 exits as
/// soon as the roundtrip is done.
unsigned spin_ms() {
    const char* s = std::getenv("NROS_SPIN_MS");
    if (s == nullptr || *s == '\0') {
        return 2000;
    }
    long v = std::strtol(s, nullptr, 10);
    return v > 0 ? static_cast<unsigned>(v) : 0u;
}

int run() {
    NROS_TRY_RET(nros::init(), 1);

    rclcpp::Node node;
    NROS_TRY_RET(nros::create_node(node, "cpp_parameters"), 1);

    // The six `rcl_interfaces/srv/*` servers, under this node's FQN. Without
    // this the parameters are still in the right store — they are simply not
    // reachable from outside the image, which is what the roundtrip below
    // proves and `ros2 param get` proves the rest of.
    if (nros_cpp_register_parameter_services(node.executor_handle()) != NROS_CPP_RET_OK) {
        std::fprintf(stderr, "register parameter services failed\n");
        return 1;
    }

    // --- scalars -------------------------------------------------------------
    //
    // `declare_parameter<T>` returns the value IN EFFECT, which is upstream's
    // contract and is not the same as the default: a launch `<param>` seeds the
    // store before user code runs, and the declare adopts it. Nothing seeds
    // this standalone image, so here the code defaults win.

    const bool verbose = node.declare_parameter<bool>("verbose", false);
    const int64_t max_iters = node.declare_parameter<int64_t>("max_iters", 100);
    const double period = node.declare_parameter<double>("ctrl_period", 0.15);
    const std::string frame = node.declare_parameter<std::string>("frame_id", "base_link");

    if (verbose != false) return 3;
    if (max_iters != 100) return 3;
    if (period < 0.149 || period > 0.151) return 3;
    if (frame != "base_link") return 3;

    std::printf("Parameters: verbose=%s, max_iters=%lld, ctrl_period=%f, frame_id=%s\n",
                verbose ? "true" : "false", static_cast<long long>(max_iters), period,
                frame.c_str());

    // Read back through the out-ref channel too — same store, upstream's other
    // spelling.
    double period_out = 0.0;
    if (!node.get_parameter<double>("ctrl_period", period_out)) return 2;
    if (period_out != period) return 2;

    // --- set -----------------------------------------------------------------
    //
    // This is `ParameterServer::apply` on the Rust side, the same entry point a
    // remote `ros2 param set` reaches, so a read-only or out-of-range value is
    // refused here exactly as it is on the wire.

    if (!node.set_parameter<double>("ctrl_period", 0.05).ok()) return 4;
    if (!node.get_parameter<double>("ctrl_period", period_out)) return 4;
    if (period_out < 0.049 || period_out > 0.051) return 4;

    if (!node.set_parameter<std::string>("frame_id", "map").ok()) return 4;
    std::string frame_out;
    if (!node.get_parameter<std::string>("frame_id", frame_out)) return 4;
    if (frame_out != "map") return 4;

    std::printf("After set: ctrl_period=%f frame_id=%s\n", period_out, frame_out.c_str());

    // --- absence is absence --------------------------------------------------

    if (node.has_parameter("missing")) return 5;
    bool tmp = false;
    if (node.get_parameter<bool>("missing", tmp)) return 5;
    // An undeclared name is not writable either (issue 1151) — a set does not
    // invent a slot.
    if (node.set_parameter<bool>("missing", true).ok()) return 5;

    // --- a sequence parameter ------------------------------------------------
    //
    // `nros::Seq<T, N>` is a VALUE with `N` inline slots and no heap — the
    // freestanding stand-in for `std::vector<T>` at this surface. The ELEMENTS
    // are owned by the store, so the `Seq` below does not have to outlive the
    // call, and `ros2 param get` sees the array.

    nros::Seq<double, 8> weights =
        node.declare_parameter("mpc_weights", nros::Seq<double, 8>{1.5, 2.5, 3.5});
    if (!node.has_parameter("mpc_weights")) return 6;
    if (weights.size() != 3) return 6;
    if (weights[0] != 1.5 || weights[1] != 2.5 || weights[2] != 3.5) return 6;

    // Bounds: reading into a too-small `Seq` is refused, not truncated. A short
    // weight matrix is a plausible wrong answer, which is worse than an error.
    nros::Seq<double, 2> too_small;
    if (node.get_parameter("mpc_weights", too_small)) return 7;

    if (!node.set_parameter("mpc_weights", nros::Seq<double, 8>{4.0, 5.0, 6.0, 7.0}).ok()) return 8;
    if (!node.get_parameter("mpc_weights", weights)) return 8;
    if (weights.size() != 4 || weights[3] != 7.0) return 8;

    // `Seq<T, N>` is itself bounded: an over-capacity `push_back` is a no-op,
    // never UB.
    nros::Seq<double, 2> bounded;
    if (!bounded.push_back(1.0) || !bounded.push_back(2.0)) return 8;
    if (bounded.push_back(3.0)) return 8;
    if (bounded.size() != 2) return 8;

    std::printf("OK mpc_weights[0]=%f n=%zu\n", weights[0], weights.size());

    // Answer the parameter services for a while, so the `ros2 param` commands
    // in the file comment have something to talk to.
    const unsigned budget = spin_ms();
    for (unsigned waited = 0; waited < budget && rclcpp::ok(); waited += 100) {
        (void)nros::spin_once(100);
    }

    rclcpp::shutdown();
    return 0;
}

} // namespace

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;
    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline.
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    std::setvbuf(stdout, nullptr, _IOLBF, 0);
#endif
    return run();
}

NROS_APP_MAIN_REGISTER()
