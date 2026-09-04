// phase-206 W1 — an override must not cost the baseline.
//
// `session.cpp` used to SELECT one of three config sources with a three-way
// ternary, so a user who set `CYCLONEDDS_URI` (or the Kconfig blob) to state one
// thing — a peer list, say — silently discarded everything they had not stated:
// the `<Threads>` stack sizes, the `<Sizing>` receive buffers, the
// `<Internal><MultipleReceiveThreads>` choice. On FreeRTOS and ThreadX those
// stack sizes are why `recv` survives a real ROS payload, so the loss is a stack
// overflow reintroduced by a config change that mentions neither.
//
// This test pins the fix at both ends:
//
//   * the CONTROL — the user fragment alone, which is exactly what the old
//     ternary handed Cyclone — must NOT carry the baseline's settings. Without
//     it, every assertion below could be passing on Cyclone's own defaults.
//   * the COMPOSITION — baseline + that same user fragment must carry BOTH: the
//     baseline's threads and buffers, and the user's discovery settings.
//
// The reading is Cyclone's own config introspection (`<Tracing>` with the
// `config` category), which prints every RESOLVED config item after all sources
// have been merged. That is the value the DDSI stack will actually use, not a
// restatement of the string we passed in — "the image booted" would prove
// nothing here, since it boots either way.

#define NROS_RMW_CYCLONEDDS_TEST_BASELINE 1
#include "cyclone_config.hpp"

#include <dds/dds.h>

#include <cstdio>
#include <cstring>
#include <string>
#include <unistd.h>
#include <vector>

// `nros_test_domain.h` also carries the suite's `take`/`take_request` span
// adapters, so it needs the vtable ABI in scope even where — as here — nothing
// drives a vtable.
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"

#include "nros_test_domain.h"

using nros_rmw_cyclonedds::compose_cyclone_config;
using nros_rmw_cyclonedds::kCycloneConfigMax;
using nros_rmw_cyclonedds::kEmbeddedCycloneConfig;

namespace {

int g_failures = 0;

void fail(const char* what) {
    std::fprintf(stderr, "FAIL: %s\n", what);
    g_failures++;
}

void check(bool cond, const char* what) {
    if (!cond) {
        fail(what);
    }
}

/// A user config that states ONLY discovery — nothing the baseline states.
///
/// This is the shape the defect is about: an override that has no opinion on
/// threads or buffers, and used to silently drop both.
const char* kUserDiscoveryOnly = "<CycloneDDS><Domain Id=\"any\">"
                                 "<Discovery>"
                                 "<ParticipantIndex>auto</ParticipantIndex>"
                                 "<MaxAutoParticipantIndex>33</MaxAutoParticipantIndex>"
                                 "<Peers><Peer Address=\"10.10.0.7\"/></Peers>"
                                 "</Discovery>"
                                 "</Domain></CycloneDDS>";

/// Bring a domain up on `config` and return every `config:` line Cyclone logged.
///
/// Empty means the domain never came up, which every caller treats as a failure
/// rather than as "no settings found" — an absent dump would otherwise make the
/// negative control pass for the wrong reason.
std::vector<std::string> resolved_config(const char* config, uint32_t domain_id, const char* tag) {
    char path[256];
    std::snprintf(path, sizeof(path), "nros_cyclone_config_%s_%d.log", tag,
                  static_cast<int>(getpid()));
    std::remove(path);

    char full[kCycloneConfigMax * 2];
    const int n = std::snprintf(full, sizeof(full),
                                "%s,<CycloneDDS><Domain Id=\"any\"><Tracing>"
                                "<Category>config</Category><OutputFile>%s</OutputFile>"
                                "</Tracing></Domain></CycloneDDS>",
                                config, path);
    std::vector<std::string> lines;
    if (n < 0 || static_cast<size_t>(n) >= sizeof(full)) {
        fail("tracing fragment did not fit");
        return lines;
    }

    const dds_entity_t domain = dds_create_domain(domain_id, full);
    if (domain < 0) {
        std::fprintf(stderr, "dds_create_domain(%s) failed: %s\n", tag, dds_strretcode(-domain));
        return lines;
    }
    (void)dds_delete(domain);

    std::FILE* f = std::fopen(path, "r");
    if (f == nullptr) {
        std::fprintf(stderr, "no config dump at %s\n", path);
        return lines;
    }
    char buf[2048];
    while (std::fgets(buf, sizeof(buf), f) != nullptr) {
        const char* marker = std::strstr(buf, "config: ");
        if (marker != nullptr) {
            lines.emplace_back(marker + std::strlen("config: "));
        }
    }
    std::fclose(f);
    std::remove(path);
    return lines;
}

size_t count_containing(const std::vector<std::string>& lines, const char* needle) {
    size_t n = 0;
    for (const std::string& l : lines) {
        if (l.find(needle) != std::string::npos) {
            n++;
        }
    }
    return n;
}

bool has(const std::vector<std::string>& lines, const char* needle) {
    return count_containing(lines, needle) > 0;
}

// ---------------------------------------------------------------------------
// The composition helper itself.
// ---------------------------------------------------------------------------

void test_compose_joins_and_skips() {
    char out[64];
    const char* all[3] = {"a", "b", "c"};
    check(compose_cyclone_config(out, sizeof(out), all, 3), "compose(a,b,c) failed");
    check(std::strcmp(out, "a,b,c") == 0, "compose(a,b,c) != \"a,b,c\"");

    // An unset override contributes nothing — not an empty token.
    const char* sparse[3] = {"a", "", nullptr};
    check(compose_cyclone_config(out, sizeof(out), sparse, 3), "compose(a,\"\",null) failed");
    check(std::strcmp(out, "a") == 0, "empty/null fragments were not skipped");

    const char* tail[3] = {nullptr, "", "z"};
    check(compose_cyclone_config(out, sizeof(out), tail, 3), "compose(null,\"\",z) failed");
    check(std::strcmp(out, "z") == 0, "leading empties left a separator");

    // Order is precedence, and precedence is the point: the user's fragment must
    // be LAST, because Cyclone lets a later token override an earlier one.
    const char* pair[2] = {kEmbeddedCycloneConfig, kUserDiscoveryOnly};
    char big[kCycloneConfigMax];
    check(compose_cyclone_config(big, sizeof(big), pair, 2), "compose(baseline,user) failed");
    check(std::strncmp(big, kEmbeddedCycloneConfig, std::strlen(kEmbeddedCycloneConfig)) == 0,
          "composed string does not START with the baseline");
    const size_t blen = std::strlen(big);
    const size_t ulen = std::strlen(kUserDiscoveryOnly);
    check(blen > ulen && std::strcmp(big + blen - ulen, kUserDiscoveryOnly) == 0,
          "composed string does not END with the user fragment");
}

void test_compose_overflow_fails_loud() {
    // Truncation is the one outcome that must never happen: a clipped config is
    // unterminated XML, and Cyclone would report it as a parse error against a
    // string the user never wrote.
    char tiny[4];
    const char* frags[2] = {"abc", "def"};
    check(!compose_cyclone_config(tiny, sizeof(tiny), frags, 2),
          "overflow returned true instead of failing");

    // Exactly-fits is not an overflow: "ab,cd" plus its terminator is 6 bytes.
    char exact[6];
    const char* fits[2] = {"ab", "cd"};
    check(compose_cyclone_config(exact, sizeof(exact), fits, 2), "exact fit rejected");
    check(std::strcmp(exact, "ab,cd") == 0, "exact fit produced the wrong string");

    char one_short[5];
    check(!compose_cyclone_config(one_short, sizeof(one_short), fits, 2),
          "one byte short was accepted");
}

// ---------------------------------------------------------------------------
// What Cyclone actually resolves.
// ---------------------------------------------------------------------------

/// CONTROL: the user fragment alone — what the old ternary passed — carries none
/// of the baseline. If this ever starts passing the baseline assertions, the
/// composition test below has stopped proving anything.
void test_user_alone_loses_the_baseline(uint32_t domain_id) {
    const std::vector<std::string> cfg = resolved_config(kUserDiscoveryOnly, domain_id, "control");
    if (cfg.empty()) {
        fail("control: no resolved config (domain did not come up)");
        return;
    }
    check(!has(cfg, "Threads/Thread/StackSize"),
          "control: baseline thread stacks present without the baseline");
    check(!has(cfg, "MultipleReceiveThreads/#text: false"),
          "control: MultipleReceiveThreads=false present without the baseline");
    check(!has(cfg, "Sizing/ReceiveBufferSize/#text: 64 KiB"),
          "control: 64 KiB receive buffer present without the baseline");
    // The control still has to be a working config, or it proves nothing.
    check(has(cfg, "Discovery/Peers/Peer[@Address]: 10.10.0.7"),
          "control: the user's own peer did not resolve");
}

/// THE DEFECT: baseline + a user fragment that states only <Discovery>.
void test_composition_keeps_both(uint32_t domain_id) {
    const char* frags[3] = {kEmbeddedCycloneConfig, "", kUserDiscoveryOnly};
    char composed[kCycloneConfigMax];
    if (!compose_cyclone_config(composed, sizeof(composed), frags, 3)) {
        fail("composition: compose_cyclone_config failed");
        return;
    }

    const std::vector<std::string> cfg = resolved_config(composed, domain_id, "composed");
    if (cfg.empty()) {
        fail("composition: no resolved config (domain did not come up)");
        return;
    }

    // The baseline survives an override that never mentioned it. Five threads
    // are named there: dq.builtins / recv / dq.user at 64 KiB, tev / gc at 16.
    check(count_containing(cfg, "Threads/Thread/StackSize/#text: 64 KiB") == 3,
          "composition: the three 64 KiB thread stacks did not survive");
    check(count_containing(cfg, "Threads/Thread/StackSize/#text: 16 KiB") == 2,
          "composition: the two 16 KiB thread stacks did not survive");
    check(has(cfg, "Sizing/ReceiveBufferSize/#text: 64 KiB"),
          "composition: the baseline receive buffer did not survive");
    check(has(cfg, "Internal/MultipleReceiveThreads/#text: false"),
          "composition: MultipleReceiveThreads=false did not survive");

    // And the user still gets what they asked for.
    check(has(cfg, "Discovery/Peers/Peer[@Address]: 10.10.0.7"),
          "composition: the user's peer was lost");
    check(has(cfg, "Discovery/MaxAutoParticipantIndex/#text: 33"),
          "composition: the user's MaxAutoParticipantIndex was lost");
}

} // namespace

int main() {
    test_compose_joins_and_skips();
    test_compose_overflow_fails_loud();

    const uint32_t domain = nros_test_domain(61);
    test_user_alone_loses_the_baseline(domain);
    test_composition_keeps_both(domain);

    if (g_failures != 0) {
        std::fprintf(stderr, "%d check(s) failed\n", g_failures);
        return 1;
    }
    std::printf("OK\n");
    return 0;
}
