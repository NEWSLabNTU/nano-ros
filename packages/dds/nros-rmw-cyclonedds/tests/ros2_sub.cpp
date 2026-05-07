// Phase 117.12.A — nano-ros subscriber binary for stock-RMW interop
// E2E. Subscribes to `chatter` (std_msgs/msg/String), waits up to
// 10 s for the first sample, then prints the data field on stdout
// and exits. Companion `ros2_pubsub_e2e.sh` runs `ros2 topic pub`
// against this binary and asserts the captured data field matches.

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
} // namespace

extern "C" nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        return 1;
    }

    nros_rmw_session_t s{};
    s.node_name  = "ros2_sub";
    s.namespace_ = "/";
    uint32_t domain = 0;
    if (const char *e = std::getenv("ROS_DOMAIN_ID")) {
        domain = static_cast<uint32_t>(std::atoi(e));
    }
    if (g_vt->open(nullptr, 0, domain, s.node_name, &s) != NROS_RMW_RET_OK) {
        return 2;
    }

    nros_rmw_qos_t qos = NROS_RMW_QOS_PROFILE_DEFAULT;
    nros_rmw_subscriber_t sub{};
    sub.topic_name = "chatter";
    sub.type_name  = "std_msgs::msg::dds_::String_";
    sub.qos        = qos;
    if (g_vt->create_subscriber(&s, sub.topic_name, sub.type_name, "",
                                0, &qos, &sub) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_subscriber failed\n");
        return 3;
    }

    uint8_t buf[256] = {};
    const auto deadline = std::chrono::steady_clock::now() +
                          std::chrono::seconds(10);
    while (std::chrono::steady_clock::now() < deadline) {
        if (g_vt->has_data(&sub)) {
            int32_t n = g_vt->try_recv_raw(&sub, buf, sizeof(buf));
            if (n > 0) {
                // CDR-LE std_msgs/msg/String:
                //   [0..4)  encap
                //   [4..8)  string length (incl NUL) LE
                //   [8..)   chars + NUL
                if (n < 9) {
                    std::fprintf(stderr, "short payload: %d bytes\n", n);
                    return 4;
                }
                uint32_t slen = static_cast<uint32_t>(buf[4]) |
                    (static_cast<uint32_t>(buf[5]) <<  8) |
                    (static_cast<uint32_t>(buf[6]) << 16) |
                    (static_cast<uint32_t>(buf[7]) << 24);
                if (slen == 0 || slen > static_cast<uint32_t>(n) - 8) {
                    std::fprintf(stderr, "bad string length %u\n", slen);
                    return 5;
                }
                // Strip trailing NUL.
                std::printf("DATA=%.*s\n",
                            static_cast<int>(slen - 1),
                            reinterpret_cast<const char *>(buf + 8));
                std::fflush(stdout);
                g_vt->destroy_subscriber(&sub);
                (void) g_vt->close(&s);
                return 0;
            }
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(20));
    }
    std::fprintf(stderr, "timeout waiting for sample\n");
    g_vt->destroy_subscriber(&sub);
    (void) g_vt->close(&s);
    return 6;
}
