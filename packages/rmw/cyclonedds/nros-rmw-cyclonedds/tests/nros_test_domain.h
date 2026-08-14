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

#ifndef NROS_TEST_DOMAIN_H
#define NROS_TEST_DOMAIN_H

#include <cstdint>
#include <cstdlib>
#include <unistd.h>

static inline uint32_t nros_test_domain(uint32_t fallback) {
    (void)fallback;
    if (const char *e = std::getenv("ROS_DOMAIN_ID")) {
        return static_cast<uint32_t>(std::atoi(e));
    }
    // 1..=232: 0 is where everything that never thought about it lands, and the
    // ROS 2 range tops out well below 255.
    return static_cast<uint32_t>((static_cast<unsigned>(getpid()) % 232u) + 1u);
}

#endif // NROS_TEST_DOMAIN_H
