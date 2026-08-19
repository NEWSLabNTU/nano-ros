// issue 0580 — the domain a self-contained cyclone test should run on.
//
// A DDS domain is a shared bus. Every test in this directory used to name one
// with a literal (99 six times, 88 once, 42 once), so two concurrent runs of
// the suite discovered each other's writers and read the result as a delivery
// bug rather than a collision. Reproduced by running two full suites at once:
// both failed, each having captured the OTHER's payload.
//
// Resolution order:
//   1. `ROS_DOMAIN_ID` when set — pinning one is how you reproduce a failure by
//      hand, and it is what the ros2-interop scripts export so their nano-ros
//      helper binary lands on the same bus as the `ros2` CLI.
//   2. otherwise a per-PROCESS domain, so concurrent runs cannot overlap.
//
// The `fallback` argument is kept in the signature but is deliberately NOT the
// default any more: it documents which tests historically shared a bus (all the
// 99s were one group) without reintroducing the sharing.
//
// Safe for the multi-session tests here (`service_concurrent` opens two): every
// session in ONE process resolves to the same value. It would NOT be safe for a
// test that pairs two PROCESSES and needs them to meet — those are the
// `ros2_*` binaries, and they take the domain from the environment their shell
// script exports (case 1 above).
//
// Mirrors `nros_tests::unique_ros_domain_id` (packages/testing/nros-tests/src/lib.rs)
// and `ros2_e2e_common.sh`'s `nros_unique_ros_domain_id` — one scheme, three
// languages, rather than a third invention.

// issue 0703 — the ceiling is 101, and it is not a style choice.
//
// Cyclone (RTPS) derives its ports arithmetically from the domain:
// `7400 + 250*D` for multicast discovery, `+10 + 2*participantIndex` for
// unicast. Linux hands out ephemeral ports from 32768 (the default
// `/proc/sys/net/ipv4/ip_local_port_range` floor), and
// `7400 + 250*102 = 32900` is inside that range — so from domain 102 up, the
// port a participant MUST have is one the OS may already have given to any
// other process on the box. The bind fails outright:
//
//     ddsi_udp_create_conn: failed to bind to ANY:44900: address in use
//
// which surfaces as `create_session` returning an error, i.e. a test that
// "fails" for reasons having nothing to do with what it tests. The rate rises
// with how many ephemeral ports are in use, which is why this was ~2-in-5
// inside `just check` and 0-in-4 solo, on a different test each time (issue
// 0703's whole symptom). Measured, with 32768-34000 held: D=100 rc=0, D=101
// rc=0, D=102 rc=2, D=103 rc=2.
//
// 101 is the last safe value with margin for the per-participant offsets:
// `7400 + 250*101 + 11 + 2*9 = 32679`. It is also exactly the range ROS 2
// documents as safe on Linux, so a value from here is one a user could
// legally have set by hand.

#ifndef NROS_TEST_DOMAIN_H
#define NROS_TEST_DOMAIN_H

// The modulus, shared by all three assigners (Rust, shell, C++).
#define NROS_TEST_DOMAIN_MAX 101u

#include <cstdint>
#include <cstdlib>
#include <unistd.h>

static inline uint32_t nros_test_domain(uint32_t fallback) {
    (void)fallback;
    if (const char *e = std::getenv("ROS_DOMAIN_ID")) {
        return static_cast<uint32_t>(std::atoi(e));
    }
    // 1..=101: 0 is where everything that never thought about it lands, and 101
    // is the last domain whose RTPS ports stay below the ephemeral range (see
    // NROS_TEST_DOMAIN_MAX above).
    return static_cast<uint32_t>((static_cast<unsigned>(getpid()) % NROS_TEST_DOMAIN_MAX) + 1u);
}

#endif // NROS_TEST_DOMAIN_H
