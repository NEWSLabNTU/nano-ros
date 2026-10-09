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
//
// That claim was false between issues 0707 and 0747: 0707 taught only the Rust
// assigner to probe and step. All three probe again as of 0747. The FIRST
// candidate still differs by design — Rust partitions slots into blocks, these
// two fold a slot-or-pid — but "do not take a bus somebody is already on" is now
// common to all three, which is the part that was load-bearing.

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

// issue 0747 — probe before taking the bus, the way the Rust assigner does.
//
// Issue 0707 taught `nros_tests::unique_ros_domain_id` to read /proc/net/udp and
// step past an occupied domain. That fix landed in the Rust assigner ONLY, and
// this header's claim of "one scheme, three languages" quietly stopped being
// true. The cost was measured on 2026-08-21: `check-rmw-cyclonedds` went 3 red
// in ~6 in-sweep `just check` runs and 0 red in 3 solo runs, a different test
// each time, with the reason printed once —
//
//     [5] ddsi_udp_create_conn: failed to bind to ANY:8650: address in use
//     open failed
//
// Domain 5, i.e. 7400 + 250*5. Not issue 0703 (that was domains >= 102 reaching
// the ephemeral range, and its ceiling still holds): a plain collision with
// whatever else on the box picked the same number. `open failed` is
// `create_session` returning non-OK, so the test dies before doing anything it
// was written to test, which is why the failing test rotated.
//
// Blind picking is not merely unlucky here. `getpid() % 101` has 101 buckets,
// `just check` fans out 32-way over hundreds of short-lived processes, and Linux
// hands out PIDs sequentially — so any two participants whose PIDs differ by a
// multiple of 101 land on the same bus. A collision inside one sweep is the
// expected case.
#include <cerrno>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fcntl.h>
#include <initializer_list>
#include <sys/file.h>
#include <sys/stat.h>
#include <unistd.h>

/// Is a DDS participant already bound to this domain's SPDP discovery port?
///
/// Mirrors `nros_tests::domain_discovery_port_busy`: RTPS derives its ports from
/// the domain arithmetically, so "somebody is on domain d" is answerable locally
/// without joining the bus — every participant there binds `7400 + 250*d`.
/// /proc/net/udp's second column is `HEXADDR:HEXPORT`.
///
/// Answers `false` when the tables cannot be read (a non-Linux host, a container
/// without /proc): unknown must not read as busy, or every domain looks taken
/// and the stepping below degrades to its fallback for no reason.
static inline bool nros_test_domain_busy(uint32_t domain) {
    const unsigned long want = 7400UL + 250UL * static_cast<unsigned long>(domain);
    for (const char *table : {"/proc/net/udp", "/proc/net/udp6"}) {
        std::FILE *f = std::fopen(table, "r");
        if (f == nullptr) {
            continue;
        }
        char line[512];
        bool first = true;
        while (std::fgets(line, sizeof(line), f) != nullptr) {
            if (first) { // header row
                first = false;
                continue;
            }
            // `  sl  local_address ...` — skip sl, then read HEX:HEX.
            const char *p = line;
            while (*p == ' ') p++;
            while (*p != '\0' && *p != ' ') p++;   // sl (ends with ':')
            while (*p == ' ') p++;
            const char *colon = std::strchr(p, ':');
            if (colon == nullptr) {
                continue;
            }
            unsigned long port = std::strtoul(colon + 1, nullptr, 16);
            if (port == want) {
                std::fclose(f);
                return true;
            }
        }
        std::fclose(f);
    }
    return false;
}

/// Issue 1762 — CLAIM a domain for the rest of this process, or report that
/// another process holds it.
///
/// The busy probe above reads who has BOUND a port, and two concurrent callers
/// have bound nothing yet when each probes the other's choice, so stepping
/// alone lets them converge (measured in the Rust assigner: two slots, one
/// domain). An exclusive non-blocking `flock` on
/// `${TMPDIR:-/tmp}/nros-test-domain-claims/<d>.lock` — the SAME files the Rust
/// and shell assigners lock — breaks the tie. The descriptor is deliberately
/// never closed: the claim lasts as long as the process can be on the bus, and
/// the kernel drops it on exit however the process ends.
///
/// Answers "claimed" when no claim can be MADE (no writable temp dir): what
/// cannot be seen must not be invented, the probe's own rule.
static inline bool nros_test_domain_claim(uint32_t domain) {
    const char *tmp = std::getenv("TMPDIR");
    if (tmp == nullptr || *tmp == '\0') {
        tmp = "/tmp";
    }
    char dir[512];
    char path[600];
    std::snprintf(dir, sizeof(dir), "%s/nros-test-domain-claims", tmp);
    if (mkdir(dir, 0777) != 0 && errno != EEXIST) {
        return true;
    }
    std::snprintf(path, sizeof(path), "%s/%u.lock", dir, static_cast<unsigned>(domain));
    const int fd = open(path, O_WRONLY | O_CREAT | O_CLOEXEC, 0666);
    if (fd < 0) {
        return true;
    }
    if (flock(fd, LOCK_EX | LOCK_NB) != 0) {
        const bool held = errno == EWOULDBLOCK;
        close(fd);
        return !held;
    }
    return true; // fd kept open on purpose: it IS the claim
}

static inline uint32_t nros_test_domain(uint32_t fallback) {
    (void)fallback;
    if (const char *e = std::getenv("ROS_DOMAIN_ID")) {
        // An explicit pin is never stepped away from. Reproducing a failure by
        // hand means being ON that bus, and the ros2-interop scripts export it
        // precisely so their helper binary meets the `ros2` CLI there.
        return static_cast<uint32_t>(std::atoi(e));
    }
    // 1..=101: 0 is where everything that never thought about it lands, and 101
    // is the last domain whose RTPS ports stay below the ephemeral range (see
    // NROS_TEST_DOMAIN_MAX above).
    // Issue 1762 — every session in one process must resolve to the SAME
    // domain (the header's contract above), and the claim below would refuse
    // this process its own domain on a second call. So the first answer is
    // kept. (Per translation unit: a static in a `static inline` function is
    // one per TU; each test here is one TU.)
    static uint32_t claimed = 0;
    if (claimed != 0) {
        return claimed;
    }
    const uint32_t first =
        static_cast<uint32_t>((static_cast<unsigned>(getpid()) % NROS_TEST_DOMAIN_MAX) + 1u);
    // Step, bounded, and keep the determinism where it is still free: with
    // nothing squatting the answer is bit-identical to the old scheme, and it
    // moves only where reusing the domain would be wrong. A candidate is taken
    // only once CLAIMED (issue 1762). Giving up returns the first candidate — a
    // box where every domain looks busy is not something this function can fix,
    // and returning nothing would break every caller (issue 0707's contract,
    // same words).
    for (uint32_t step = 0; step < NROS_TEST_DOMAIN_MAX; step++) {
        const uint32_t candidate = ((first - 1u + step) % NROS_TEST_DOMAIN_MAX) + 1u;
        if (!nros_test_domain_busy(candidate) && nros_test_domain_claim(candidate)) {
            claimed = candidate;
            return candidate;
        }
    }
    return first;
}

#endif // NROS_TEST_DOMAIN_H

/* phase-406 W2 — the copying takes carry ONE `rmw_mut_byte_span_t` where they
 * used to carry `buf` / `cap` / `*out_len`. These adapters keep the old flat
 * call shape for the tests below, which are transport round-trip tests: what
 * they assert is that bytes arrive, not how the byte range is spelled at the
 * ABI. Keeping the shape here means the span change is reviewed in ONE place
 * instead of being retyped into eleven `if` conditions, where a mistake would
 * read as a transport failure. */
static inline rmw_ret_t nros_test_take(const nros_rmw_vtable_t* vt,
                                       const rmw_subscription_t* sub, uint8_t* buf, size_t cap,
                                       size_t* out_len, bool* taken) {
    rmw_mut_byte_span_t span{buf, cap, 0};
    const rmw_ret_t rc = vt->take(sub, &span, taken);
    if (out_len != nullptr) *out_len = span.len;
    return rc;
}

static inline rmw_ret_t nros_test_take_request(const nros_rmw_vtable_t* vt,
                                               const rmw_service_t* srv, uint8_t* buf, size_t cap,
                                               int64_t* seq_out, size_t* out_len, bool* taken) {
    rmw_mut_byte_span_t span{buf, cap, 0};
    const rmw_ret_t rc = vt->take_request(srv, &span, seq_out, taken);
    if (out_len != nullptr) *out_len = span.len;
    return rc;
}

static inline rmw_ret_t nros_test_take_response(const nros_rmw_vtable_t* vt,
                                                const rmw_client_t* cli, uint8_t* buf, size_t cap,
                                                int64_t* seq_out, size_t* out_len, bool* taken) {
    rmw_mut_byte_span_t span{buf, cap, 0};
    const rmw_ret_t rc = vt->take_response(cli, &span, seq_out, taken);
    if (out_len != nullptr) *out_len = span.len;
    return rc;
}
