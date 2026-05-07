// Phase 117.12.B — nano-ros service server binary for stock-RMW
// interop E2E. Hosts `/add_two_ints` (example_interfaces/srv/
// AddTwoInts), waits up to 30 s for a request, replies with `sum =
// a + b`, then exits. Companion `ros2_srv_e2e.sh` drives `ros2
// service call` against this binary and asserts the reply value.

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
    s.node_name  = "ros2_srv_server";
    s.namespace_ = "/";
    uint32_t domain = 0;
    if (const char *e = std::getenv("ROS_DOMAIN_ID")) {
        domain = static_cast<uint32_t>(std::atoi(e));
    }
    if (g_vt->open(nullptr, 0, domain, s.node_name, &s) != NROS_RMW_RET_OK) {
        return 2;
    }

    nros_rmw_service_server_t srv{};
    srv.service_name = "add_two_ints";
    srv.type_name    = "example_interfaces::srv::dds_::AddTwoInts";
    if (g_vt->create_service_server(&s, srv.service_name, srv.type_name, "",
                                    domain, &srv) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_service_server failed\n");
        return 3;
    }

    const auto deadline = std::chrono::steady_clock::now() +
                          std::chrono::seconds(30);
    while (std::chrono::steady_clock::now() < deadline) {
        if (g_vt->has_request(&srv)) {
            uint8_t rbuf[64] = {};
            int64_t seq = -1;
            int32_t n = g_vt->try_recv_request(&srv, rbuf, sizeof(rbuf), &seq);
            if (n > 0) {
                int64_t a = get_le64(rbuf + 4);
                int64_t b = get_le64(rbuf + 12);
                uint8_t reply[12] = {0x00, 0x01, 0x00, 0x00};
                put_le64(reply + 4, a + b);
                if (g_vt->send_reply(&srv, seq, reply, sizeof(reply))
                    != NROS_RMW_RET_OK) {
                    std::fprintf(stderr, "send_reply failed\n");
                    return 4;
                }
                std::printf("REPLIED a=%lld b=%lld sum=%lld\n",
                            static_cast<long long>(a),
                            static_cast<long long>(b),
                            static_cast<long long>(a + b));
                std::fflush(stdout);
                g_vt->destroy_service_server(&srv);
                (void) g_vt->close(&s);
                return 0;
            }
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(20));
    }
    std::fprintf(stderr, "timeout waiting for request\n");
    g_vt->destroy_service_server(&srv);
    (void) g_vt->close(&s);
    return 5;
}
