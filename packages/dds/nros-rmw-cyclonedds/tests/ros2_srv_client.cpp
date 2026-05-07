// Phase 117.12.B — nano-ros service client binary for stock-RMW
// interop E2E. Calls `/add_two_ints` (example_interfaces/srv/
// AddTwoInts) once with `(a=11, b=31)`, prints `SUM=<value>` on
// stdout, then exits. Companion `ros2_srv_e2e.sh` runs
// `ros2 run demo_nodes_cpp add_two_ints_server` on the same domain
// and asserts the printed value is `42`.

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <thread>

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"

namespace {
const nros_rmw_vtable_t *g_vt = nullptr;

void put_le64(uint8_t *out, int64_t v) {
    for (int i = 0; i < 8; ++i) {
        out[i] = static_cast<uint8_t>((v >> (i * 8)) & 0xff);
    }
}
int64_t get_le64(const uint8_t *in) {
    int64_t v = 0;
    for (int i = 0; i < 8; ++i) {
        v |= static_cast<int64_t>(in[i]) << (i * 8);
    }
    return v;
}
} // namespace

extern "C" nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        std::fprintf(stderr, "register failed\n");
        return 1;
    }

    nros_rmw_session_t s{};
    s.node_name  = "ros2_srv_client";
    s.namespace_ = "/";
    uint32_t domain = 0;
    if (const char *e = std::getenv("ROS_DOMAIN_ID")) {
        domain = static_cast<uint32_t>(std::atoi(e));
    }
    if (g_vt->open(nullptr, 0, domain, s.node_name, &s) != NROS_RMW_RET_OK) {
        return 2;
    }

    nros_rmw_service_client_t cli{};
    cli.service_name = "add_two_ints";
    cli.type_name    = "example_interfaces::srv::dds_::AddTwoInts";
    if (g_vt->create_service_client(&s, cli.service_name, cli.type_name, "",
                                    domain, &cli) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_service_client failed\n");
        return 3;
    }

    // Discovery delay so the stock server's reader matches our
    // request writer + response reader before the call fires.
    std::this_thread::sleep_for(std::chrono::seconds(2));

    uint8_t req[20] = {0x00, 0x01, 0x00, 0x00};
    put_le64(req + 4,  11);
    put_le64(req + 12, 31);
    uint8_t rep[64] = {};
    int32_t n = g_vt->call_raw(&cli, req, sizeof(req), rep, sizeof(rep));
    if (n <= 0) {
        std::fprintf(stderr, "call_raw returned %d\n", n);
        g_vt->destroy_service_client(&cli);
        (void) g_vt->close(&s);
        return 4;
    }
    int64_t sum = get_le64(rep + 4);
    std::printf("SUM=%lld\n", static_cast<long long>(sum));
    std::fflush(stdout);

    g_vt->destroy_service_client(&cli);
    (void) g_vt->close(&s);
    return 0;
}
