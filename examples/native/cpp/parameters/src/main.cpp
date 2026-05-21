/// @file main.cpp
/// @brief C++ parameters example — exercises nros::ParameterServer.
///
/// Declares bool / int64 / double / string parameters, reads them back,
/// updates a value, and prints the results. Used by the
/// `cpp_parameters` integration test (Phase 117.9).

#include <cstdio>
#include <cstdlib>
#include <cstring>

#include <nros/parameter.hpp>
#include <nros/result.hpp>

namespace {

int run() {
    nros::ParameterServer<8> params;

    if (!params.declare_parameter<bool>("use_sim_time", true).ok()) {
        std::fprintf(stderr, "declare bool failed\n");
        return 1;
    }
    if (!params.declare_parameter<int64_t>("max_iters", 100).ok()) {
        std::fprintf(stderr, "declare int failed\n");
        return 1;
    }
    if (!params.declare_parameter<double>("ctrl_period", 0.15).ok()) {
        std::fprintf(stderr, "declare double failed\n");
        return 1;
    }
    if (!params.declare_parameter<const char*>("frame_id", "base_link").ok()) {
        std::fprintf(stderr, "declare string failed\n");
        return 1;
    }

    if (params.parameter_count() != 4) {
        std::fprintf(stderr, "expected 4 params, got %zu\n", params.parameter_count());
        return 1;
    }

    bool use_sim_time = false;
    int64_t max_iters = 0;
    double ctrl_period = 0.0;
    char frame_id[64] = {0};

    if (!params.get_parameter<bool>("use_sim_time", use_sim_time).ok()) return 2;
    if (!params.get_parameter<int64_t>("max_iters", max_iters).ok()) return 2;
    if (!params.get_parameter<double>("ctrl_period", ctrl_period).ok()) return 2;
    if (!params.get_parameter("frame_id", frame_id, sizeof(frame_id)).ok()) return 2;

    if (use_sim_time != true) return 3;
    if (max_iters != 100) return 3;
    if (ctrl_period < 0.149 || ctrl_period > 0.151) return 3;
    if (std::strcmp(frame_id, "base_link") != 0) return 3;

    if (!params.set_parameter<double>("ctrl_period", 0.05).ok()) return 4;
    if (!params.get_parameter<double>("ctrl_period", ctrl_period).ok()) return 4;
    if (ctrl_period < 0.049 || ctrl_period > 0.051) return 4;

    if (!params.set_parameter<const char*>("frame_id", "map").ok()) return 4;
    if (!params.get_parameter("frame_id", frame_id, sizeof(frame_id)).ok()) return 4;
    if (std::strcmp(frame_id, "map") != 0) return 4;

    if (params.has_parameter("missing")) return 5;

    bool tmp = false;
    nros::Result missing = params.get_parameter<bool>("missing", tmp);
    if (missing.ok()) return 5;

    std::printf("OK use_sim_time=%d max_iters=%lld ctrl_period=%f frame_id=%s\n",
                static_cast<int>(use_sim_time),
                static_cast<long long>(max_iters), ctrl_period, frame_id);
    return 0;
}

} // namespace

int main() {
    return run();
}
