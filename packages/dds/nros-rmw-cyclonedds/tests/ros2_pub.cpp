// Phase 117.12.A — nano-ros publisher binary for stock-RMW interop
// E2E. Publishes a `std_msgs/msg/String { data: "hello-from-nros" }`
// 10× at 100 ms intervals, then exits. Companion `ros2_pubsub_e2e.sh`
// captures the output of `ros2 topic echo` and asserts the data
// field is delivered byte-equal.

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
        std::fprintf(stderr, "register failed\n");
        return 1;
    }

    nros_rmw_session_t s{};
    s.node_name  = "ros2_pub";
    s.namespace_ = "/";
    uint32_t domain = 0;
    if (const char *e = std::getenv("ROS_DOMAIN_ID")) {
        domain = static_cast<uint32_t>(std::atoi(e));
    }
    if (g_vt->open(nullptr, 0, domain, s.node_name, &s) != NROS_RMW_RET_OK) {
        return 2;
    }

    nros_rmw_qos_t qos = NROS_RMW_QOS_PROFILE_DEFAULT;

    nros_rmw_publisher_t pub{};
    pub.topic_name = "chatter";
    pub.type_name  = "std_msgs::msg::dds_::String_";
    pub.qos        = qos;
    if (g_vt->create_publisher(&s, pub.topic_name, pub.type_name, "",
                               0, &qos, &pub) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_publisher failed\n");
        return 3;
    }

    // Discovery delay so a stock subscriber on the same domain
    // matches before we publish.
    std::this_thread::sleep_for(std::chrono::seconds(2));

    // CDR-LE for std_msgs::msg::dds_::String_ { data = "hello-from-nros" }
    //   00 01 00 00      encap = CDR_LE
    //   <len:u32-LE>     "hello-from-nros\0".size  (16, including NUL)
    //   <chars> 00       payload (15 chars + NUL)
    const char *msg = "hello-from-nros";
    uint32_t mlen = static_cast<uint32_t>(std::strlen(msg) + 1);
    uint8_t cdr[64] = {
        0x00, 0x01, 0x00, 0x00,
        static_cast<uint8_t>(mlen & 0xff),
        static_cast<uint8_t>((mlen >> 8) & 0xff),
        static_cast<uint8_t>((mlen >> 16) & 0xff),
        static_cast<uint8_t>((mlen >> 24) & 0xff),
    };
    std::memcpy(cdr + 8, msg, mlen);
    size_t cdr_len = 8 + mlen;

    for (int i = 0; i < 50; ++i) {
        nros_rmw_ret_t r = g_vt->publish_raw(&pub, cdr, cdr_len);
        if (r != NROS_RMW_RET_OK) {
            std::fprintf(stderr, "publish_raw[%d] = %d\n", i,
                         static_cast<int>(r));
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }

    g_vt->destroy_publisher(&pub);
    (void) g_vt->close(&s);
    std::printf("OK\n");
    return 0;
}
